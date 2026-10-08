# Multi Launcher — Plan C: Mouse Coordinate Inspector & Crosshair

**Codex implementation plan · Approved scope**

**Required working branch:** `mouse-improvements` (Git does not permit spaces in branch names)  
**Target:** Windows 10/11, Rust, eframe/egui, Win32, local-only

> **Required directive:** **Use the project’s active checkpoint commit cadence and define the task-specific commit boundaries in the plan.**
>
> **Current Git authority:** The user explicitly superseded publication requirements: "Don't worry about pushes. Just commit to the new branch locally." Use local checkpoint commits on `mouse-improvements`. Do not push, merge, rebase, force-push, squash, amend, or rewrite checkpoint history without a new explicit request.

## 0. Mission and execution contract

Deliver **one integrated coordinate utility** whose independent operating modes are:

1. **Coordinate Inspector** — `coord` opens/toggles a small, responsive, movable-by-cursor-offset, click-through HUD displaying live mouse position and selected coordinate space. The HUD supports compact and detailed presentation, freeze/unfreeze, copying, last-copied status, monitor indication, and contextual help.
2. **One-shot coordinate picker** — `coord pick` enters an **explicit active capture session**. A left click captures the pixel at the cursor and copies it to the clipboard **without passing the capture click into the underlying application**. Escape cancels and must not change clipboard contents. Successful capture ends Pick mode without leaving an overlay or lingering mouse/key interception.
3. **Independent crosshair** — `crosshair` toggles a native, passive, click-through crosshair independently of the HUD. Support a small centered crosshair, optional virtual-desktop-spanning horizontal/vertical guides, saved color/thickness/arm-length/opacity, and high-contrast visibility.

The HUD and crosshair must be usable **separately and simultaneously**. Multiple invocations must be deterministic and idempotent (toggle or focus as defined below), never create duplicate workers or windows. The tool must never produce significant continuous overhead when inactive.

### Hard rules

- Verify the current branch before every commit. Never commit on `master`.
- Keep history readable: no meaningless WIP commits, giant accumulated unrelated changes, or forced commits just to fill stage IDs.
- Review each coherent checkpoint diff, commit locally on `mouse-improvements`, then proceed.
- Remediation from later tests or review gets a distinct descriptive follow-up commit.
- Commit bodies may explain preserved behavior, native input semantics, or compatibility concerns.
- Preserve all existing user changes. Do not merge, rebase, force-push, or rewrite history.

## Build, testing and runtime efficiency

Windows build and test cycles can be long. Make substantive progress before expensive full verification. Use pure unit tests, source inspection, formatting and focused checks where useful. Run meaningful native compilation and targeted Cargo Nextest around integration/verification checkpoints, not before every commit. Avoid concurrent expensive builds.

For long-running jobs use completion signals where available; otherwise inspect at a slow approximate 10–20 minute cadence. Report honest test results. Distinguish automated evidence from user-performed Windows smoke tests if no interactive Windows desktop is available.

Especially verify capture clicks never reach underlying applications; Escape preserves clipboard contents; no stuck click/hook; mixed DPI and negative monitor coordinates; passive foreground typing; independent HUD/crosshair toggles; Screen Draw emergency hotkeys, radial quick tap/hold, OCR/Color Pick/MkMacro coexistence; no background work while off.

## Completion criteria and report

Proceed autonomously through bounded milestones with planning, implementation and independent review roles. Respect Git safety and environment limits. At every coherent checkpoint review the diff, verify the branch, and commit locally.

Report:

- Base master hash, feature branch and upstream (if any).
- Chronological checkpoint subjects and short hashes; publication intentionally omitted by user instruction.
- Delivered features and material plan deviations.
- Focused build/test commands and real results; manual acceptance passed versus blocked/unperformed.
- Reviewer findings and remediation commits.
- Final Git status, known limitations and precise branch review steps.

Do not claim completion solely from checked milestone boxes. Prove accepted behavior as far as the environment permits, and explicitly identify any remaining unverified native behavior.

## Execution state

- Base master: `9b4687b53d06d5ac09226cbe0421edae77312fc0`.
- Branch: `mouse-improvements`; clean local branch created from synchronized master.
- Upstream: none; no publication required by latest user instruction.
- Repository instructions and agent definitions read; bounded read-only architecture inventory complete.
- Implementation milestones and acceptance matrix: source-grounded planning below; ordinary unspecified defaults are implementation decisions, not additional approved requirements.

## M1-A: Typed coordinate model and preferences (`implemented; verification pending`)

Objective: feature-owned pure coordinate types, settings and conversion/formatting; no workers or GUI behavior changes yet.

Ownership: `src/coordinate_tool/{mod,model,settings}.rs`, exported by `src/lib.rs`. Central typed, serde-defaulted settings in `src/settings/model.rs` supply one preference source to runtime and commands; no parallel configuration file.

Required behavior:

- Signed physical desktop coordinates; monitor-local coordinates relative to monitor bounds; foreground-client coordinates relative to the Win32 client origin, with an explicit unavailable result.
- Checked arithmetic and a single sample containing desktop point, monitor identity/bounds/work area and client origin. HUD and copy use the same sample.
- Freeze preserves the sample. Copy emits `x,y` in the selected space; unavailable coordinates never silently yield zero or stale clipboard success.
- HUD and crosshair enabled states are independent. Enabled/frozen/last-successfully-copied values are transient.
- Defaults: desktop space, compact HUD, cursor offset (16,24) physical pixels; red crosshair, thickness 2, arm length 12, opacity 1, guides off, high-contrast outline on.
- Normalize thickness to 1..16, arm length 2..256, opacity to .1..1, offset to -512..512. Clamp HUD placement to the cursor monitor work area.

Invariants: legacy settings deserialize with defaults; no native work or clipboard effects in this checkpoint. Non-goals: command/plugin/native/input/GUI integration.

Tests: signed/negative coordinates, absent client origin, overflow, frozen sample stability, independent state transitions, legacy/partial settings defaults and normalization/round-trip.

Verification: format and inspect the scoped diff at this checkpoint; run the named coordinate model/settings Nextest tests at the integration verification checkpoint to avoid repeated expensive builds. Done: types/preferences integrated, meaningful tests written and subsequently passing at the verification gate.

## Control and coordinate contract

All commands are exact and case-insensitive. The passive HUD remains click-through; controls are available through launcher commands rather than passive global input interception.

- `coord`: toggle HUD; `coord on|off`: explicit HUD state.
- `coord space desktop|monitor|client`: select signed physical-pixel coordinate space.
- `coord compact|detailed`; `coord offset <signed-x> <signed-y>`: presentation and cursor offset.
- `coord freeze|unfreeze`: preserve/resume HUD sample; `coord copy`: copy selected sample as `x,y`.
- `coord pick`: start idempotent active one-shot capture; `coord cancel`: cancel capture; `coord help`: contextual help.
- `crosshair`: independent toggle; `crosshair on|off`: explicit state.
- `crosshair color <#rrggbb>`; `crosshair thickness <1..16>`; `crosshair length <2..256>`; `crosshair opacity <0.1..1.0>`.
- `crosshair guides on|off`; `crosshair contrast on|off`; `crosshair help`.

Client space means foreground-window client coordinates in physical pixels, including negative points outside its client area. Missing geometry is an explicit error. Pick captures a live click-time sample even if the HUD is frozen; it uses the selected space, copies once after paired release/native teardown, and restores prior independent HUD/crosshair state. Escape/cancel or invalid samples preserve clipboard contents. The word "pixel" identifies the cursor's selected location; the clipboard payload is coordinate text, not color or an image.

## M1-B: Passive native runtime (`implemented; verification pending`)

Objective/owner: feature-owned `coordinate_tool::{controller,native}` supplies live HUD and independent crosshair; GUI remains an adapter. Depends on M1-A.

Current seams: Screen Draw's native overlay and gesture hint overlay demonstrate layered passive surfaces; `platform::pixels::premultiplied_bgra` is shared. Do not reuse Screen Draw-specific z-order or lifecycle ownership.

Required changes:

1. Injectable sampler for cursor, virtual desktop, cursor monitor/work area and foreground-client geometry; per-monitor-aware thread context restored by RAII.
2. One runtime with distinct passive HUD/crosshair surfaces; topmost/tool/no-activate/transparent layered windows and transparent hit testing. Never focus them or install passive hooks/hotkeys.
3. Compact space/coordinate presentation; detailed desktop point, monitor identity, origins/context, freeze, last-copy and help hint. Freeze sample data while placement can follow cursor.
4. Small centered crosshair plus optional signed virtual-desktop guides, saved color/thickness/length/opacity and contrasting outline.
5. Bounded responsive sampling only while needed; skip unchanged uploads and avoid full-desktop bitmap rebuilding on each small cursor move. Handle geometry/DPI changes and unavailable samples explicitly.
6. Stop workers/timers when no mode/capture needs them; release native/GDI resources before acknowledging teardown.

Tests: injectable lifecycle counts, repeated toggles, independent/both modes, freeze, failures, geometry changes, disposal, unchanged rendering, idle sampler call counts, pure guide/outline geometry. No pixel snapshot campaign.

Invariants: passive input/focus untouched; signed coordinates; no duplicate workers/windows; no periodic work while off. Non-goals: capture, clipboard, commands or redesign of adjacent tools.

Verification at integration gate: targeted `coordinate_tool` Nextest plus `cargo check --lib`; directly affected helper consumers only if shared mechanics are extracted. Done: runtime integrated, lifecycle/resource/idle tests meaningful and passing at verification gate, scoped diff reviewed.

## M1-C: Launcher controls and persistence (`pending`)

Objective/ownership: typed commands dispatch, plugin discovers, feature controller executes, GUI adapter coordinates feedback. Depends on M1-B.

Scope: command model/parser/bus/handler/host/headless paths; `gui/command_host.rs`, new GUI adapter, app construction/render/shutdown; plugin module and built-in registration.

Required changes:

1. Typed validated operations for the control contract, exhaustive canonical dispatch and UI-required headless results.
2. One plugin for exact coordinate/crosshair prefixes, command inventory and help; malformed queries/lookalikes never activate anything and discovery has no side effects.
3. One controller construction/settings reload/event polling/shutdown path. Persist through Settings transactions and publish new preferences only after successful write.
4. Inject clipboard writes through existing `actions::clipboard::set_text`; last-copy success changes only after successful write. Frozen copy matches displayed sample.
5. Exclude these operations from generic query overrides and unintended hide/refocus policies. Help does not mutate runtime state.
6. Defer exposing pick/cancel activation until M2-B rather than advertise no-op commands.

Tests: exact parsing/invalid values, plugin inventory, metadata, bus once, headless, query-override policy, persistence/clipboard failure, freeze agreement and independent toggles.

Verification at integration gate: focused feature/plugin/parser/handler/GUI filters; `cargo nextest run --test domain command_bus_architecture`; `cargo check --lib`. Invariants: existing commands/plugins preserved. Non-goals: capture/global passive shortcuts/settings UI redesign. Done: all passive controls reachable, saved correctly, copy/help and lifecycle integrated, scoped diff reviewed.

## M2-A: Capture owner and paired-input state machine (`pending`)

Objective/ownership: one feature-owned transient capture session with capture-scoped native interception. Depends on M1-C. Scope: new capture model and native/controller extensions.

Required changes:

1. Explicit arming/capture/paired-release drain/cancellation/teardown/completion states and session identity; repeated start preserves active session; stale results cannot publish.
2. Capture-only mouse interception and Escape handling. Sample physical position/context at left down; swallow both down and matching up, even if cancellation occurs between them.
3. Activation with pre-held buttons must wait for a fresh pair, not capture the triggering input. Ordinary keys, wheel, other buttons and emergency chords pass through.
4. Gesture suppression lease only during capture; no preference resetting. Native callbacks only bounded state updates, no clipboard calls.
5. Terminal outcomes only after native teardown; partial setup failure, cancellation, channel failure and shutdown follow the same cleanup owner. Restore prior independent passive states.

Invariants: neither half of successful pick leaks; Escape never changes clipboard; no stale results/resources. Non-goals: pixel colors/images/screenshots, synthetic replacement input, arbitrary synchronization sleeps, permanent hooks, unrelated hook resets.

Tests: down/up, mid-pair cancellation, pre-held activation, Escape, pass-through input, duplicate start/stale generation, installation failure/teardown order, suppression release, four prior passive combinations.

Verification at integration gate: coordinate-tool Nextest and native `cargo check --lib`; real input delivery proof in M3. Done: scoped capture compiled/tested, completion follows cleanup, scoped diff reviewed.

## M2-B: Pick completion and coexistence (`pending`)

Objective: canonical pick/cancel commands, clipboard publication after teardown, and mutually exclusive capture admission. Depends on M2-A.

Ownership: GUI adapter arbitrates launcher-owned capture conflicts; feature controller owns coordinate capture. Scope: coordinate commands/plugin/GUI plus directly relevant Color Pick/OCR/Screen Draw admission points and actual MkMacro/radial callers where necessary.

Required changes:

1. Expose pick/cancel/help; copy exact selected-space click-time `x,y` once after cleanup. Clipboard/geometry failure reports error without success status.
2. Reject pick before mutation while Color Pick/OCR/Screen Draw exclusive sessions run; reciprocal guards prevent their start during pick.
3. Passive HUD/crosshair remain compatible. Preserve emergency recovery, radial dispatch, launcher invocation/hotstrings and MkMacro before/after capture; adjust only demonstrated conflicts.
4. Cancel/disable/shutdown cannot lose paired release or publish stale outcomes; keep query/history/focus behavior explicit and scoped.

Tests: exact copy after cleanup, clipboard sentinel on cancel/failure, reciprocal conflicts, relevant activation routes, passive coexistence and reopen. Verification: feature and modified GUI lifecycle/suppression filters plus `cargo check --lib`. Non-goals: general interaction manager, broad hook refactor/historical acceptance migration. Done: canonical pick integrated, failures recover, conflicts covered and scoped diff reviewed.

## M3-A: Verification, documentation and independent review (`pending`)

Objective: targeted integration evidence, focused native smoke, one independent review and user documentation. Depends on M2-B. Resolve concrete findings in separate descriptive remediation commits, rerunning only affected checks.

Use a small controlled native receiver fixture if useful to prove real input delivery. Synthetic input is allowed in test tooling; production input workarounds are not. No full suite, historical qualification campaign, repeated review loops or screenshots required. Record unavailable hardware/desktop conditions honestly.

Acceptance matrix:

| Accepted behavior | Required evidence |
| --- | --- |
| Exact families, typed dispatch, headless boundary | Plugin/parser/bus/host tests and domain architecture test |
| Signed desktop/monitor/client points | Conversion tests and native cursor/geometry comparison |
| Mixed DPI | Per-monitor-aware seam plus actual mixed-DPI smoke when available |
| HUD compact/detail, offset, monitor/help | Controller tests and focused visual smoke |
| Freeze/copy/last-copy | Frozen sample/injected clipboard tests and real clipboard smoke |
| Saved styles/legacy settings | Defaults/normalization/round-trip tests and restart smoke |
| HUD alone/crosshair alone/both/repetition | Lifecycle tests and native window smoke |
| Guides/opacity/contrast | Pure geometry and focused visual smoke |
| Passive typing/clicks | Native receiver observes unchanged ordinary input |
| Pick pair swallowed | State-machine tests and receiver sees neither transition |
| Escape preserves clipboard | Sentinel test and native smoke |
| Cancel/failure allows input/reopen | Lifecycle tests and post-capture receiver input |
| No duplicate windows/workers/hooks | Lifecycle counts and actual teardown inspection |
| Idle no continuous work | Backend call counts and stopped worker/timer/window inspection |
| Screen Draw emergency recovery | Scoped test/smoke with passive modes and capture conflict test |
| Radial/OCR/Color Pick/MkMacro coexistence | Relevant conflict/dispatch tests and before/after native sequence |
| Complete ownership/integration | Cumulative diff, stale references and independent review |

Each row must have actual evidence or be explicitly recorded as unresolved environmental acceptance. Pure tests cannot establish real mixed-DPI or native click delivery. Do not claim complete feature acceptance while required behavior remains unverified.

## Checkpoint subjects and verification ledger

Use `feat(coord): [M1-A] establish coordinate model and preferences`, `[M1-B] add passive HUD and crosshair runtime`, `[M1-C] integrate launcher controls and persistence`, `[M2-A] add scoped coordinate capture lifecycle`, `[M2-B] integrate capture and coexistence guards`, and `test(coord): [M3-A] verify Windows capture and document controls` when those coherent changes are ready. Exact subjects may adapt to real changes.

M1-A source checkpoint committed `c15d546f`: signed/checked conversion, samples, independent transient state, frozen copy formatting, placement and normalized persisted preferences. Settings editor preserves preferences.

M1-B source checkpoint implemented: lazy single worker, injectable sampling/backend, per-monitor-aware native sampling, cached four-surface renderer, narrow guides, display refresh and teardown. Sampling errors clear the live sample; last-good geometry is placement-only. Native client context uses the last external window while launcher owns foreground; help must state this explicitly.

Rustfmt and diff checks passed for M1-A/M1-B; Cargo verification and native acceptance have not yet run. M1-C next. Subsequent checkpoints: pending. Commit completion and acceptance verification are tracked separately: source checkpoints may precede expensive tests, but milestones are complete only after their scoped acceptance checks pass.
