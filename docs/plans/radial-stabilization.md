# Radial stabilization ledger

Status: **S0 complete — source/diff audit only.** No reported bug is marked
fixed, no native pass is claimed, and S1 has not started. This ledger records
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
| S1 | `pending` | Focused hotkey/visibility and Designer/runtime close lifecycle repairs and gate. |
| S2 | `pending` | Tooltip units and stable hover/native presentation repairs and gate. |
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
