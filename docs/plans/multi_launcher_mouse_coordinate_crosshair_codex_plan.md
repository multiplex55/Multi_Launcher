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

## M1-A: Typed coordinate model and preferences (`pending`)

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

Later checkpoint handoffs and acceptance matrix are being completed before dependent implementation starts.
