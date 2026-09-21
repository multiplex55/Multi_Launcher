# Radial stabilization ledger

Status: **S5 in_progress — independent-review remediation batch.**
S0–S4 remain complete; no native pass is claimed. This ledger records
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
| S3 | `complete` | Slot-first compact Designer layout/interaction repairs and gate. |
| S4 | `complete` | Shared unflattened Cascade layers, exact-frame ancestor navigation, and parity across runtime/native preview/embedded preview; focused gate and post-removal checks passed. |
| S5 | `in_progress` | Review remediation batch: runtime relocation, shared visible-stack/placement policy, complete ancestor gesture, close/send-failure terminals, live-cache pruning, stale preview replies, reset epochs, and production-path coverage. Full suite/native acceptance remain deferred. |

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

### S5 review-remediation findings and active-job record

S5 begins from clean committed HEAD `bb01565f5fc8585f5e3953e19b10360279382da2`.
The independent review identified ten bounded findings being remediated in
this batch: (P1-1) runtime Cascade relocation validates the composite against
per-frame state and can drop layers; (P1-2) runtime/native-preview/embedded
visible-stack folds diverge at mixed SameCenter/Cascade edges; (P1-3) ancestor
navigation commits on pointer-down, lacks capture, and can leak a release or
double-click tail; (P1-4) Discard close can retry Stop after delivery failure;
(P1-5) runtime Close send failure does not feed the correlated handoff terminal;
(P2-6) Cascade placement differs between runtime/native preview/embedded;
(P2-7) popped frame caches are retained; (P2-8) embedded delayed replies can
resurrect a popped child; (P2-9) Reset does not clear retained tree collapse
state; and (P2-10) production-path Designer/layered-scene/gesture coverage is
too weak. The remediation preserves SameCenter serialization/defaults, the
S1/S2/S3 lifecycle and presentation contracts, exact FrameId state, and no
per-frame native windows. Full `cargo nextest run`, native UI acceptance, and
the final S5 integration evidence are explicitly deferred to the follow-up
verification batch.

```text
Job purpose / milestone: S5 review-remediation focused Nextest gate (narrow tests already completed separately)
Command and cwd: `cargo nextest run --no-fail-fast -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'`; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; preflight every selector before its single Cargo tree
Source SHA + source/untracked diff fingerprint: bb01565f5fc8585f5e3953e19b10360279382da2; mod.rs SHA256 ED8516E87752CA4BA1B473C8B93FF665DF528E7228C1C969F51BEC797E41DDF8; preview.rs SHA256 339326964DC353BC2090D6E96EFA7015BD5B06C935FE6D64BBD3AC3CC30DCC8F; native_preview.rs SHA256 5DE598792A8AB1E8D83C337E21C64389AD7623C692E7225182C9422C619C2585; controller.rs SHA256 C2A32B289048B65BC338B0ABC2A8ECAFF5FA4D662A9476F9AF4C3E5C7479E3BA; geometry.rs SHA256 DADA4C47A1EA89BE554BAA30265655E9D37D8B9B5068ADD1E8D782F33E641806; native.rs SHA256 B987AE8EF2427CF26C833802BE5BA3BEC3A3E7B550B25A4D31E2434DC5141CFE; session.rs SHA256 DFB45B6C812EE41BF55397EC8F2331D8F7F8F1709F3FF1A42A121D42C63B3746; ledger SHA256 915E0035B1CD0DB1A34BFF557C65DDB7146CEE21560A0309754F5DAC2CFCA1B3
Session/PID + process start and identity: exec PTY/session 45836; observed Cargo PIDs 27776 (start 2026-09-18 07:28:06 -04:00) and 29304 (start 2026-09-18 07:28:07 -04:00) in the one Nextest tree; rustc PID 30288 (start 07:28:10 -04:00); process inspection did not expose parent command lines, so these are recorded as the single Cargo/Nextest tree rather than separate jobs
Durable stdout/stderr log: target/s5-remediation-focused.log
Exit-code record: target/s5-remediation-focused.exit
Launch time: 2026-09-18 07:28:06 -04:00 (PTY 45836)
First observation due: 2026-09-18 07:38:06 -04:00
Second observation due: 2026-09-18 07:53:06 -04:00
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
First observation (2026-09-18 07:38:15 -04:00): PTY 45836 and Cargo PIDs 27776/29304 remained responsive; rustc children were active (latest observed PID 7436, with 18612/23356/24476/24516/27268/27552/30192); durable log length 67 bytes and still contained only the initial compile line; no failure or exit record yet. Next observation due 2026-09-18 07:53:06 -04:00. Reattach to PTY 45836; do not launch a duplicate.
Completion (2026-09-18 07:49:14 -04:00): same PTY 45836 completed the exact selector with Nextest run ID `cbfc6bf5-9933-4966-ae96-3e67197009a3`; 153 selected tests passed, 4,448 skipped, zero failures; `target/s5-remediation-focused.exit` contains `0`; durable log is 21,877 bytes. No corrective test batch was required. The separate full suite and native UI acceptance remain deferred by this S5 packet.
Corrective narrow batch after the gate: added the reducer guard clearing a stale release-consumption latch when a new typed ancestor press begins; `cargo test session::tests --lib -- --nocapture` passed 22/22 (3,981 filtered), `cargo check` passed, `cargo fmt --all -- --check` passed, and `git diff --check` passed. Updated source fingerprint: session.rs SHA256 05DE205A866FB417B0731E338DE028E7143DA3B5AC9CADF497D5604D190FC9B7.

Corrective focused-gate record (current source after the narrow guard): command is the exact selector above, same cwd/profile and one Cargo tree; source hashes are mod.rs ED8516E87752CA4BA1B473C8B93FF665DF528E7228C1C969F51BEC797E41DDF8, preview.rs 339326964DC353BC2090D6E96EFA7015BD5B06C935FE6D64BBD3AC3CC30DCC8F, native_preview.rs 5DE598792A8AB1E8D83C337E21C64389AD7623C692E7225182C9422C619C2585, controller.rs C2A32B289048B65BC338B0ABC2A8ECAFF5FA4D662A9476F9AF4C3E5C7479E3BA, geometry.rs DADA4C47A1EA89BE554BAA30265655E9D37D8B9B5068ADD1E8D782F33E641806, native.rs B987AE8EF2427CF26C833802BE5BA3BEC3A3E7B550B25A4D31E2434DC5141CFE, session.rs 05DE205A866FB417B0731E338DE028E7143DA3B5AC9CADF497D5604D190FC9B7. Exec PTY/session 1684; Cargo PIDs 19592 and 26748 (both start 2026-09-18 07:56:23 -04:00), rustc PIDs 28500 and 29200 (start 07:56:25 -04:00); durable log `target/s5-remediation-focused-corrective.log`, exit `target/s5-remediation-focused-corrective.exit`; first observation due 2026-09-18 08:06:23 -04:00, second due 08:21:23, later +1,200 seconds.
Completion (2026-09-18 08:17:36 -04:00, recorded from the durable exit/log): Nextest run ID `cdeb277b-3918-4d53-9532-25eb6d725c69`; 153 selected tests passed, 4,448 skipped, zero failures; exit record is `0`, log length 21,877 bytes, and no Cargo process remains.
```

Post-gate checks were already run against this exact source after the final
narrow reducer guard and are corroborated by the corrective compilation: `cargo
check` exit `0`, `cargo fmt --all -- --check` exit `0`, and `git diff --check`
exit `0`. No source was edited while the corrective gate ran, and no duplicate
Cargo job was launched.

S5 remediation outcome: P1-1 now relocates only live per-frame layouts and
rebuilds the shared layered runtime scene; P1-2 uses the same
`visible_frame_ids` SameCenter-reset/Cascade-append fold in runtime, native
preview, and embedded preview; P1-3 adds typed ancestor press correlation,
Navigate-owner capture, matching-release commit, capture/move cancellation, and
double-click-tail suppression; P1-4 terminalizes every close-intent Stop
attempt, including Discard, after delivery failure; P1-5 feeds a correlated
ActionHandoff Closed event when runtime Close delivery fails; P2-6 centralizes
edge-aware Cascade candidate probing and SameCenter fallback in
`cascade_placement`; P2-7 prunes runtime/native-preview/embedded per-frame
caches and reducer frozen dynamic results to live FrameIds; P2-8 rejects stale
embedded replies by request token, editor generation, current FrameId, and
frame token; P2-9 epochs Designer tree widget IDs on Reset while preserving
ordinary expansion persistence; and P2-10 adds production-path relocation,
layered-scene, gesture, close, reset, and stale-reply regressions.

The stale-path audit found no surface caller of `cascade_candidate_centers`
outside the shared geometry helper/tests, no selected-cell Cascade placement
path in runtime/native preview/embedded, and no native ancestor route bypassing
`BeginAncestorNavigation`; `SessionEvent::NavigateToFrame` remains only for
programmatic embedded Back/navigation. Native interactive acceptance remains
unverified: the ignored `live_radial_host_probe` was not run, and no desktop
pixel/input trace or native K/D/T matrix was claimed. S5 therefore remains
`in_progress`, pending the separate full `cargo nextest run` and native
acceptance/final integration review; this remediation commit does not close
those gaps.

### S4 active-job record

The S4 implementation is being verified from committed source HEAD
2a24f2a8bd4b89d1151704b8fa1d65c1bb4ad0a1. The focused gate must use one
Cargo/Nextest process tree and no source edits while it runs. The active
identity below is completed before launch and is updated with the true exit
status and any corrective batch.

```text
Job purpose / milestone: S4 shared layered Cascade scene and exact-frame Back focused gate
Command and cwd: selector preflight; cargo nextest run --no-fail-fast -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'; then cargo check; cargo fmt --all -- --check; git diff --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; installed nextest filterset preflight before the focused batch
Source SHA + source/untracked diff fingerprint: 2a24f2a8bd4b89d1151704b8fa1d65c1bb4ad0a1 + geometry.rs 6E5AFD8C337D45445BFA1D02949078657A8010417D3966492A06EC7B36CBDF9B; render.rs FD8B529FD908D7046751782E4762763B7FB3C4591607D228374B0AE90A8F5BF3; session.rs 8BC74FBC3661AB3C09C7518164F3443A04B2B5EE0BC328DD9508D813ECC4FF50; controller.rs 56B990713B53899E721C4536F54FBDA08854E8270C0CEEA1967911E07BD0B9AA; native_preview.rs 89013E6C05CCB68A0CC21A4AD4A509ED8B93E7E60DD3BA598FDABD68A1526C74; preview.rs BC4F301F26F95C6A8DC63231C5835D1146CF03A1495827A2F1D18409C750F41E
Session/PID + process start and identity: compile preflight PTY session 48954; wrapper PID 29816 (pwsh), start 2026-09-18T02:47:08.7614974-04:00; identity target/s4-check.identity; one sequential Cargo tree
Durable stdout/stderr log: target/s4-check.log (compile preflight; authoritative gate will use target/s4-focused.log)
Exit-code record: target/s4-check.exit (compile preflight; authoritative gate will use target/s4-focused.exit)
Launch time: 2026-09-18T02:47:08.7614974-04:00
First observation due: launch + 600 seconds
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: compile preflight PTY 48954 completed exit 101 at 2026-09-18T02:48:06.8932522-04:00; one embedded drag pattern incorrectly treated InputOwner as Option. Corrective compile preflight below fixes that pattern and the now-unused child-menu parameter.
```

### S4 corrective compile preflight

```text
Job purpose / milestone: S4 compile corrective preflight after first type error
Command and cwd: cargo check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target
Source SHA + source/untracked diff fingerprint: 2a24f2a8bd4b89d1151704b8fa1d65c1bb4ad0a1 + preview.rs BC4F301F26F95C6A8DC63231C5835D1146CF03A1495827A2F1D18409C750F41E; prior five S4 fingerprints unchanged above
Session/PID + process start and identity: compile corrective PTY session 33524; wrapper PID 21180 (pwsh), start 2026-09-18T02:49:31.7373104-04:00; identity target/s4-check-corrective.identity; one sequential Cargo tree
Durable stdout/stderr log: target/s4-check-corrective.log
Exit-code record: target/s4-check-corrective.exit
Launch time: 2026-09-18T02:49:31.7373104-04:00
First observation due: launch + 600 seconds
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: compile preflight completed exit 0; the focused corrective compile is recorded below.
```

### S4 focused gate record

```text
Job purpose / milestone: S4 focused layered Cascade tests and compile/format gate
Command and cwd: cargo nextest list -p multi_launcher -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'; cargo nextest run -p multi_launcher --no-fail-fast -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'; cargo check; cargo fmt --all -- --check; git diff --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; selector preflight before one focused batch
Source SHA + source/untracked diff fingerprint: 2a24f2a8bd4b89d1151704b8fa1d65c1bb4ad0a1 + geometry.rs 32CB696D64D859F9F92404BF8692D9A418332DCD6CDF380D5D80FE91D6283B13; render.rs D5A055646B163044BC1F02DDDF08815DB08A4AF8D706192385E594D6B862B7A0; session.rs 7EEC1959863E603F3AAC128A539D2F4697C01F41268E202EE1F51E0B46F67819; controller.rs EAB51957BD684F337D780BC8E16BCC0F687DDE7E05BF861CC84C91AC48EF55A4; native_preview.rs E93E27B9BB93CBB8126B200F55F51DF50E097EDEA78CE81E6B08160BAD3E5542; preview.rs 2411051C415C677A7E89891E23E3B0BEEF636A3036F6DAA0DC5F9FEE4D8D2D3C
Session/PID + process start and identity: PTY session 96455; wrapper PID 22216 (pwsh), start 2026-09-18T02:52:55.3197544-04:00; identity target/s4-focused.identity; one sequential Cargo/Nextest tree
Durable stdout/stderr log: target/s4-focused.log
Exit-code record: target/s4-focused.exit
Launch time: 2026-09-18T02:52:55.3197544-04:00
First observation due: launch + 600 seconds
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: PTY 96455 completed at `2026-09-18T02:58:01-04:00` with compile exit 101 before the focused run; the migrated controller test still referenced removed `cascade_layout`, and the render test emitted one unused-mut warning. The test migration was corrected; the next corrective gate recorded 144/146 before the final fixture correction.
```

### S4 corrective focused gate record

```text
Job purpose / milestone: S4 corrective focused layered Cascade tests after legacy-helper test migration
Command and cwd: same selector preflight, focused --no-fail-fast batch, cargo check, cargo fmt --all -- --check, and git diff --check as the S4 focused gate above; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; selector preflight before one focused batch
Source SHA + source/untracked diff fingerprint: 2a24f2a8bd4b89d1151704b8fa1d65c1bb4ad0a1 + geometry.rs 32CB696D64D859F9F92404BF8692D9A418332DCD6CDF380D5D80FE91D6283B13; render.rs 6449228EF70917AEDE6E1AF1BC10FFBDDBEB4913B9E83A3F315B05D3545FB5D3; session.rs 7EEC1959863E603F3AAC128A539D2F4697C01F41268E202EE1F51E0B46F67819; controller.rs 3C0029151CD2833E3108B21D559D59E0E91DEFBBB625E5E0A7FC8C1B727DFC0A; native_preview.rs E93E27B9BB93CBB8126B200F55F51DF50E097EDEA78CE81E6B08160BAD3E5542; preview.rs 2411051C415C677A7E89891E23E3B0BEEF636A3036F6DAA0DC5F9FEE4D8D2D3C
Session/PID + process start and identity: PTY session 65898; wrapper PID 17772 (pwsh), start 2026-09-18T02:59:20.5494307-04:00; identity target/s4-focused-corrective.identity; one sequential Cargo/Nextest tree
Durable stdout/stderr log: target/s4-focused-corrective.log
Exit-code record: target/s4-focused-corrective.exit
Launch time: 2026-09-18T02:59:20.5494307-04:00
First observation due: launch + 600 seconds
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: PTY 65898 completed 2026-09-18T03:43:30.0263749-04:00 with selector preflight exit 0; focused Nextest ran 146 tests (144 passed, 2 failed, 4,447 skipped); cargo check, fmt check, and diff check each exited 0. Failures were stale tests expecting flattened parent cells and selected-cell Cascade fallback behavior; both are migrated below.
```

### S4 final focused gate record

```text
Job purpose / milestone: S4 final shared layered Cascade tests and compile/format gate
Command and cwd: cargo nextest list -p multi_launcher -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'; cargo nextest run -p multi_launcher --no-fail-fast -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'; cargo check; cargo fmt --all -- --check; git diff --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; selector preflight before one focused batch
Source SHA + source/untracked diff fingerprint: 2a24f2a8bd4b89d1151704b8fa1d65c1bb4ad0a1 + geometry.rs 32CB696D64D859F9F92404BF8692D9A418332DCD6CDF380D5D80FE91D6283B13; render.rs 6449228EF70917AEDE6E1AF1BC10FFBDDBEB4913B9E83A3F315B05D3545FB5D3; session.rs 7EEC1959863E603F3AAC128A539D2F4697C01F41268E202EE1F51E0B46F67819; controller.rs 3C0029151CD2833E3108B21D559D59E0E91DEFBBB625E5E0A7FC8C1B727DFC0A; native_preview.rs A33CEE82148AFEABB51BE8DFC5788EB883C4A15CED72A1CEB86C97C1308249E2; preview.rs 20965A00DA5A79625B2E5D9556ACEF8C382B3700F96162230310D57FE61CD4BB
Session/PID + process start and identity: PTY session 93337; wrapper PID 30200 (pwsh), start 2026-09-18T03:45:31.0842299-04:00; identity target/s4-focused-final.identity; one sequential Cargo/Nextest tree
Durable stdout/stderr log: target/s4-focused-final.log
Exit-code record: target/s4-focused-final.exit
Launch time: 2026-09-18T03:45:31.0842299-04:00
First observation due: launch + 600 seconds
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: PTY 93337 completed at `2026-09-18T04:16:11-04:00`; focused batch ran 146 tests (145 passed, 1 failed, 4,447 skipped); cargo check and diff check exited 0; fmt check exited 1 on one formatter-only multiline literal in the migrated native-preview test. The fixture was corrected and the final corrective gate is recorded below.
```

### S4 focused corrective gate record

The final gate exposed one stale Cascade fallback fixture.  The coordinator now
selects an inward diagonal candidate as the normal Cascade path; the fixture
was migrated to assert that shared placement while directly retaining coverage
of `PreparedPlacement::SameCenterFallback` for an impossible local anchor.
The source remains frozen during the corrective gate below.

```text
Job purpose / milestone: S4 corrective focused layered Cascade gate after edge-aware fallback fixture migration
Command and cwd: selector preflight; cargo nextest run --no-fail-fast -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'; then cargo check; cargo fmt --all -- --check; git diff --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; installed nextest filterset preflight before one focused --no-fail-fast batch
Source SHA + source/untracked diff fingerprint: 2a24f2a8bd4b89d1151704b8fa1d65c1bb4ad0a1 + native_preview.rs A835FF9AD5B25B8EECE772EC3A582C5485372E8A8662E54140AD17B4925C0540; prior S4 fingerprints unchanged
Session/PID + process start and identity: PTY session 9120; wrapper PID 24336 (pwsh), start 2026-09-18T04:39:49.3684976-04:00; identity target/s4-focused-corrective2.identity; one sequential Cargo/Nextest tree
Durable stdout/stderr log: target/s4-focused-corrective2.log
Exit-code record: target/s4-focused-corrective2.exit
Launch time: 2026-09-18T04:39:49.3684976-04:00
First observation due: launch + 600 seconds
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: isolated selector completed at `2026-09-18T09:26:43Z`; preflight listed 1 test and the focused run passed 1/1 with 4,592 skipped, exit 0. The migrated fixture passes; the full corrective gate below remains required.
```

### S4 final corrective gate record

```text
Job purpose / milestone: S4 final corrective shared layered Cascade tests and compile/format gate
Command and cwd: cargo nextest list -p multi_launcher -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'; cargo nextest run -p multi_launcher --no-fail-fast -E 'test(/radial::session/) | test(/radial::geometry/) | test(/radial::controller/) | test(/radial::render/) | test(/radial::compositor/) | test(/radial::native/) | test(/native_preview/) | test(/radial_editor::preview/)'; cargo check; cargo fmt --all -- --check; git diff --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; selector preflight before one focused --no-fail-fast batch
Source SHA + source/untracked diff fingerprint: 2a24f2a8bd4b89d1151704b8fa1d65c1bb4ad0a1 + native_preview.rs A835FF9AD5B25B8EECE772EC3A582C5485372E8A8662E54140AD17B4925C0540; prior S4 fingerprints unchanged
Session/PID + process start and identity: PTY session 71551; wrapper PID 27056 (pwsh), start 2026-09-18T05:28:38.4605661-04:00; identity target/s4-focused-corrective-final2.identity; one sequential Cargo/Nextest tree
Durable stdout/stderr log: target/s4-focused-corrective-final2.log
Exit-code record: target/s4-focused-corrective-final2.exit
Launch time: 2026-09-18T05:28:38.4605661-04:00
First observation due: launch + 600 seconds
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: focused batch completed at `2026-09-18T06:16:11-04:00`; selector preflight exit 0; 146/146 tests passed with 4,447 skipped; cargo check exit 0; fmt check exit 1 only for one multiline literal; diff check exit 0. `cargo fmt --all` and the post-format checks below completed afterward.
```

### S4 post-format validation

The only final-gate failure was formatter output for the migrated test.  The
formatter was applied after the Cargo tree exited; this changed no behavior or
test logic.

```text
Command and cwd: cargo fmt --all; cargo fmt --all -- --check; git diff --check; G:\Repos\rust\Multi_Launcher
Source fingerprint: native_preview.rs 080466F7B730A8B7A84A6E6C88AE5CF5E9782A5DC8E87234C8A4D5B8565DA019; prior S4 fingerprints unchanged
Actual result: formatter application exit 0; fmt check exit 0; diff check exit 0. The obsolete flattened helper was then removed; the post-removal check is recorded below.
```

### S4 post-removal validation

```text
Command and cwd: cargo check; cargo fmt --all -- --check; git diff --check; G:\Repos\rust\Multi_Launcher
Session: PTY 58761 for cargo check; source was frozen during the check
Source fingerprint: geometry.rs B420503DEBE522DE768860A5CABA1A23F2FA5F14547BBBD3F501B6884AADE7F5; native_preview.rs 080466F7B730A8B7A84A6E6C88AE5CF5E9782A5DC8E87234C8A4D5B8565DA019
Actual result: cargo check exit 0 (12.94s); fmt check exit 0; diff check exit 0; repository search found no `cascade_layout` or obsolete `merge_preview_resources` references under runtime/preview source. The final unused resource-flattening helper was removed after the focused gate; this check confirmed the mechanical cleanup compiles.
```

### S4 implementation and evidence

Each retained navigation frame now keeps its own complete layout, style,
resources, and typed `FrameId`.  `LayeredInput` preserves those independent
hit maps, while `SceneLayer`/`LayeredScene` composes oldest ancestor through
active child into one shared visual scene.  Runtime controller, native preview,
and embedded Designer preview all use the same scene builder; no ancestor gets
its own native window.

Cascade placement uses actual parent extents and frozen work-area direction to
choose a modest diagonal overlap.  The preparation boundary retains an explicit
SameCenter fallback for a local point that cannot fit.  Root invocation origin,
parent-frame submenu origins, exact-frame Back/pop restoration, negative/DPI
coordinates, and drag translation remain owned by the existing geometry/session
boundaries.

Layered hit ownership is frontmost-first: child footprints and gaps are
protective, actionable cells belong only to their own frame, and exposed
ancestor cells return an exact `NavigateToFrame(FrameId)` target.  The session
reducer truncates to that frame, clears transient hover/action state, and
consumes the complete pointer release so the same gesture cannot dispatch an
ancestor action.  Repeated menu IDs are disambiguated by `FrameId`.

Evidence: the final corrective selector preflight passed; focused Nextest ran
146/146 tests with 4,447 skipped; cargo check passed; formatter and diff checks
passed after the formatter-only fixture correction; the stale `cascade_layout`
path is absent; and the post-removal cargo check passed.  The isolated migrated
fallback test also passed 1/1 with 4,592 skipped.

Native acceptance remains explicitly unverified: K1–K8 (real HWND region/pixel
presentation, desktop input routing, native screenshot overlap, and native
resource teardown) were not exercised.  No native-pass claim is made; the
remaining risk is platform-specific presentation behavior beyond the shared
fake-host and production-adapter tests.

### S3 active-job record

The S3 implementation and focused gate completed from committed source HEAD
`56567765f0d6f844f9cbf3c1aa5455fc2eaf7e55`.  Source fingerprints were captured
before the authoritative Cargo gate; the wrapper records its persistent PID and
start identity before invoking Cargo and waits for that identity to be recorded
here.  There must be one Cargo/Nextest process tree only.

```text
Job purpose / milestone: S3 bounded visual-first Designer focused gate
Command and cwd: selector preflight, then cargo nextest run --no-fail-fast, cargo check, cargo fmt --all -- --check, git diff --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; selector preflight before one focused batch
Source SHA + source/untracked diff fingerprint: 56567765f0d6f844f9cbf3c1aa5455fc2eaf7e55 + mod.rs SHA256 87F8B62E3830EE26FD65A949DC79B56810656B8E852AE2FAC0DAF12636D5128B; preview.rs SHA256 EB22FFE00DE9D89808B4782EFBD9A8103F6779BF26D2C7842D97A20DA50DAEF9; settings/model.rs SHA256 6D2B959F32C6654D3740C9E87FCD0AA7331301617D0CA718FCE6DE6398DE3396
Session/PID + process start and identity: PTY session `22104`; wrapper PID `18708` (`pwsh`), start `2026-09-18T05:03:44.2269617Z`; identity persisted in target/s3-focused.identity before Cargo launch; one sequential Cargo tree
Durable stdout/stderr log: target/s3-focused.log
Exit-code record: target/s3-focused.exit
Launch time: `2026-09-18T05:03:44.2269617Z`
First observation due: `2026-09-18T05:13:44.2269617Z` (launch + 600 seconds)
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: attempt 1 completed at `2026-09-18T05:33:02.5765032Z`; selector preflight exit 0; focused `--no-fail-fast` batch ran 300 tests with 300 passed and 4,289 skipped; `cargo check` exit 0; `git diff --check` exit 0; `cargo fmt --all -- --check` exit 1 with formatter-only diffs in `mod.rs`. Test-generated `clipboard_modifiers.json` was removed, `cargo fmt --all` applied, and the corrective gate below is authoritative.
```

### S3 corrective active-job record

Formatter output was applied after the first gate exited.  The corrective run
uses the same verified selector and one sequential Cargo tree; no source edits
will occur while it runs.

```text
Job purpose / milestone: S3 corrective bounded visual-first Designer focused gate
Command and cwd: selector preflight, then cargo nextest run --no-fail-fast, cargo check, cargo fmt --all -- --check, git diff --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; selector preflight before one focused batch
Source SHA + source/untracked diff fingerprint: 56567765f0d6f844f9cbf3c1aa5455fc2eaf7e55 + mod.rs SHA256 4D2BFF8E89917D1358FD2A40AB03EBFE9557F2061B86C87B1FE9849DE5D41B93; preview.rs SHA256 EB22FFE00DE9D89808B4782EFBD9A8103F6779BF26D2C7842D97A20DA50DAEF9; settings/model.rs SHA256 6D2B959F32C6654D3740C9E87FCD0AA7331301617D0CA718FCE6DE6398DE3396
Session/PID + process start and identity: PTY session `2707`; wrapper PID `13668` (`pwsh`), start `2026-09-18T05:34:49.9776526Z`; identity persisted in target/s3-focused-corrective.identity before Cargo launch; one sequential Cargo tree
Durable stdout/stderr log: target/s3-focused-corrective.log
Exit-code record: target/s3-focused-corrective.exit
Launch time: `2026-09-18T05:34:49.9776526Z`
First observation due: `2026-09-18T05:44:49.9776526Z` (launch + 600 seconds)
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: corrective gate completed at `2026-09-18T05:57:25.7486384Z`; selector preflight exit 0; focused `--no-fail-fast` batch ran 181 tests with 181 passed and 4,408 skipped; `cargo check`, `cargo fmt --all -- --check`, and `git diff --check` each exited 0; overall exit 0. No further corrective batch remains within S3.
```

### S3 implementation and evidence

The Designer presentation now has a serde-backed `layout_version`: untouched
legacy pane defaults migrate once to a canvas-first visual workspace, while an
explicit legacy pane choice is preserved and marked current.  `Reset Designer
layout` restores only presentation preferences (pane visibility/widths,
sections, mode, zoom/pan, skins view, and saved window geometry); the draft,
selection, undo history, assets, bindings, and radial document remain owned by
the authoring session.

The viewport body allocates a bounded top-down control/status region and a
pane-width plan derived from the actual remaining rect.  Optional tree and
inspector panes are hidden by default, each receives its own vertical scroll
region when enabled, and side panes yield width to a positive canvas at compact
sizes.  Wrapped controls and named breadcrumbs keep navigation reachable at
720×520 and 900×650 without forcing a larger window.  Tree sections remain
closed until explicitly expanded; user names keep full-name hover access while
ring IDs are secondary detail.  The existing `CanvasTransform` remains the
single drawing/hit/drag/pan/DPI boundary, and preview controls stay UI-only.

Focused regression coverage verifies canvas-first defaults, bounded compact
pane sums, UI-only reset on a dirty selected draft, legacy preference
migration/preservation, explicit tree expansion for accessibility, existing
direct-edit and preview boundaries, settings-editor preservation, and native
preview invariants.  The corrective gate selected 181 tests and passed all of
them (4,408 skipped); the first broader selector attempt also passed 300 tests
before exposing only formatter output, which was applied before the
authoritative rerun.  No native desktop execution or screenshot/high-DPI
acceptance was performed; native D1–D13 remain explicitly unverified.

The post-gate pane-gap correction was checked separately because it changed
only the bounded width arithmetic after the authoritative run.  Its durable
targeted check is recorded below.

Final post-inspection source fingerprint after the explicit-section opening
correction: `mod.rs` SHA256
`EC343D869A28A606CDF9BB26BDEB628CCA12702EE251CDB60091730BFFFA72BE`.

### S3 post-gate fix check

```text
Job purpose / milestone: S3 targeted verification of one-pane/two-pane width correction
Command and cwd: cargo nextest list/run -p multi_launcher --no-fail-fast -E 'test(/^gui::radial_editor::tests::(default_designer_layout_is_canvas_first_and_panes_fit_compact_windows|resetting_designer_layout_does_not_touch_dirty_document_or_selection)$/)'; cargo fmt --all -- --check; git diff --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; nonempty selector preflight before focused batch
Source SHA + source/untracked diff fingerprint: 56567765f0d6f844f9cbf3c1aa5455fc2eaf7e55 + mod.rs SHA256 27E47E3A4948AA84BCC76F24024FF676E34228906568AB27D50FA485B909B939
Session/PID + process start and identity: PTY session `91629`; wrapper PID `12740` (`pwsh`), start `2026-09-18T05:59:39.1685628Z`; identity persisted in target/s3-fix-check.identity before Cargo launch; one sequential Cargo tree
Durable stdout/stderr log: target/s3-fix-check.log
Exit-code record: target/s3-fix-check.exit
Launch time: `2026-09-18T05:59:39.1685628Z`
First observation due: `2026-09-18T06:09:39.1685628Z` (launch + 600 seconds)
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: targeted check completed at `2026-09-18T06:25:06.1330124Z`; selector preflight exit 0; 2 tests passed and 4,587 skipped; fmt check and diff check exit 0; overall exit 0. No further corrective batch remains within S3.
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
