# MkMacro Recording and Send Keys Initiative Ledger

Status values: `pending`, `in_progress`, `complete`, `blocked`.

This ledger implements the attached September 9, 2026 initiative on branch `recorder`.
Construction is intentionally front-loaded; broad Nextest execution begins after production
integration. A milestone is complete only after its acceptance criteria and verification succeed.

## M1 — Authoritative keyboard domain and persisted recorder settings

Status: `in_progress`

Implementation checkpoint: production integration completed; `cargo check`,
`cargo check --tests`, `cargo fmt --all --check`, and `git diff --check` passed.
Behavioral completion remains gated on M7.

Objective: establish shared typed foundations for every later recorder pass.

- Add `mkmacro::keyboard` as the owner of Windows key metadata, display names, modifier
  classification, event conversion, key inventory, and stable chord dedupe.
- Expand `MkKey` additively with supported Windows keys and a serializable raw-VK fallback
  retaining VK, scan code, and extended state. Preserve legacy serde shapes.
- Route SendInput, recorder conversion, hotkey state checks, validation, and GUI capture
  through shared mapping while retaining domain-specific macro-hotkey policy.
- Add `MkRecorderSettings` beneath `MkMacroSettings`; keep transient control suppression out
  of persistence. Bump schema 13 to 14 with an explicit additive migration.

Acceptance: F1-F24, modifiers, navigation/editing/system/lock, numpad, OEM, browser/media,
launch, and raw keys round-trip/map correctly; F25-F35 are not advertised as sendable; raw
recordings preserve scan/extended metadata; ordered chords release in reverse and clean up after
failure; modifier-only actions and a searchable/raw picker work; multi-key Down/Up is rejected;
v13 migrates without macro-content changes.

Verification: compile-level checks during construction; detailed tests in M7.

## M2 — Ordered recorder processing and generalized controls

Status: `in_progress`

Implementation checkpoint: production integration completed; `cargo check`,
`cargo check --tests`, focused hook/hotkey/processor tests, formatting, and diff checks passed.
Primary-only control filtering was removed after inspection; explicit fired occurrences are the
sole suppression path. Behavioral completion remains gated on M7.

Depends on: M1.

- Introduce one blocking recorder processor with ordered, acknowledged Pause/Resume/Finish
  boundaries; preserve RecorderRuntime lifecycle ownership and cheap hook callbacks.
- Publish cached status so snapshots do no enrichment; use one event-ordering/time domain and
  session-relative dropped counts.
- Carry `RecordingTarget { macro_id, after_step_id }` from Start through the result.
- Generalize one hotkey worker for Toggle/PauseResume/Marker with independent edges.
- Suppress exact recorder-control chords, including modifiers, without deleting later input.

Acceptance: no enrichment on egui frames, no busy polling, no Stop/Pause tail race, stable
session settings, complete control suppression, and preserved playback/recording exclusion.

Verification: compile boundary plus processor/hook/lifecycle/control tests in M7.

## M3 — Enriched event and pure semantic recording pipeline

Status: `in_progress`

Implementation checkpoint: semantic IR/translation/passes are integrated; `cargo check`,
`cargo check --tests`, nine focused semantic tests, formatting, and diff checks passed after
remediating the initial read-only findings. Behavioral completion remains gated on M7.

Depends on: M1-M2.

- Keep `recorder.rs` responsible for literal chronology and geometric normalization.
- Add explicit enriched-event/plan/provenance layers and independently testable pure passes.
- Add injectable keyboard-layout translation, per-key reconstruction, AltGr/dead-key
  conservatism, tap/hold/repeat, chord detection, text folding, delay cleanup, mouse cleanup,
  and semantic window activation.
- Keep permanent step ID allocation out of recording plans and Review.

Acceptance: malformed transitions never panic; taps/holds/chords/text/auto-repeat remain faithful;
idle delay cleanup preserves hold/movement duration; existing sampling/RDP/click/drag/scroll remains;
window actions are visible in the plan rather than hidden in final materialization.

Verification: construction `cargo check`; semantic synthetic-event tests in M7.

## M4 — Smart observations and reviewable suggestions

Status: `in_progress`

Implementation checkpoint: production WinEvent, process/window identity, clipboard, UIA,
marker/annotation, and suggestion infrastructure is integrated. Native callbacks are bounded,
auxiliary timestamps share the pause-adjusted recorder clock, UIA uses a persistent COM-owned
worker with cached element/ancestor inspection, and launch/dialog correlation is conservative and
SHOW-epoch aware. `cargo check`, `cargo check --tests`, focused observer/processor/suggestion/
window/UIA suites, formatting, and diff checks passed after independent review. Behavioral
completion remains gated on M7.

Depends on: M2-M3.

- Extend the Windows boundary with transient process/thread/window identity, a one-time baseline,
  lightweight event observations, cached metadata resolution, and pure launch/dialog correlation.
- Inspect UIA only for relevant clicks on its background boundary; never auto-generate UIA actions.
- Read clipboard only around recognized paste candidates, use redacted Debug, and retain dynamic
  Hotkey behavior unless Freeze Paste is explicitly enabled.
- Represent marker/annotation boundaries and map them to metadata.
- Discover repeat, launch/wait/activate, and freeze transformations as confidence-rated suggestions
  with source spans, descriptions, and rationales.

Acceptance: no frame/process polling or hook clipboard/UIA work; ambiguous destructive suggestions
default disabled; no invented arguments; waits default to 10 seconds; sensitive data is transient.

Verification: fake observer/clipboard/UIA and pure suggestion tests in M7.

## M5 — Recording Review, anchored apply, and local editing

Status: `in_progress`

Implementation checkpoint: the transient Review session, generated/editable modes, cached
statistics, suggestion controls, selection/trim/delete, local history, typed Action Editor target,
queued result lifecycle, captured-anchor recovery, and one document insertion transaction with
staleness-safe undo/redo are integrated. Toolbar, hotkey, and command-dispatch Stop paths publish
to Review without mutating or saving the draft. `cargo check`, `cargo check --tests`, focused
Review/anchor/queue/editor/authoring tests, formatting, and diff checks passed after independent
review. Behavioral completion remains gated on M7.

Depends on: M3-M4.

- Add focused `recording_review.rs`; GUI owns transient `RecordingReviewSession`.
- Generated mode deterministically rebuilds from cleanup plus enabled suggestions. First manual
  edit/trim freezes into editable mode; Reconfigure explicitly discards edits after confirmation.
- Cache statistics; support multi-select, delete, trim, typed action editing, and local undo/redo.
- Apply once through `editor_mutation::insert_fragment` at the captured anchor with fresh IDs.
  Missing anchor/macro preserves Review and requires explicit recovery.
- Applying is one document undo transaction; Cancel never mutates draft/store.

Acceptance: both Stop paths only open Review; literal view is immutable; provenance/confidence and
dropped warnings are visible; review edits do not dirty the macro; insertion remains anchored when
selection changes; transient sensitive data clears after Apply/Cancel.

Verification: review session and authoring integration tests in M7.

## M6 — Ephemeral preview and complete UI integration

Status: `in_progress`

Implementation checkpoint: M6 production integration is implemented and passes formatting,
all-target compilation, and focused preview/Review/controller/hotkey/recorder tests. Preview
tickets retain exact terminal state across later runs, Stop finalizes asynchronously through one
owned Review queue, and Record Options are draft-owned. M7 remains the owner of broad behavioral
suite consolidation, so M6 stays `in_progress` until that downstream acceptance pass.

Depends on: M2-M5.

- Add ephemeral precompiled-program execution that never saves/publishes the draft but uses the
  normal compiler, executor, admission, controls, diagnostics, and input cleanup.
- Preview all or the min-through-max selected range; Stop uses existing runtime control.
- Bind Record Options to draft recorder settings and group controls; add Pause/Resume, Marker,
  Annotate, Review, and Preview states.
- Route toolbar Stop and global-hotkey pending results through one Review opening path.
- Update MkMacro help/README for recording cleanup and physical keys versus Unicode Text.

Acceptance: preview cannot overlap recording/playback, does not persist/mutate macro content,
and cleans input; annotation-owned pause resumes correctly while manual pause does not; no second
insertion or executor path remains.

Verification: fake-runtime and toolbar/controller tests in M7.

## M7 — Consolidated behavioral test authoring and migration

Status: `in_progress`

Implementation checkpoint: keyboard/input/action-editor and schema/settings coverage is migrated
and green (29 focused Nextest cases). The batch also fixed round-before-threshold delay cleanup,
F24 hotkey admission, annotation prompt-key release suppression, transient clipboard replacement
redaction, and an existing-application launch false positive. Recorder lifecycle/input safety is also
green (23 focused and 40 affected-module Nextest cases), including partial Unicode cleanup, queued
hook-tail fencing, partial hook-install rollback, transactional Pause/Resume failure recovery,
terminal Stop and Shutdown serialization, same-runtime processor recovery, idempotent
Stop-for-Review transfer, hotkey refresh
edges, cached snapshots, and session-scoped callback drops.
Semantic recording, mouse/window authoring, and smart observation/suggestion coverage is green (more than 100
affected library cases, 7 recorder integration cases, and all 9 authoring integration cases). This
batch removed the obsolete literal-plan compatibility path and fixed long-held modifier shortcut
classification, precise Freeze Paste targeting, balanced cross-Pause mouse ownership, per-primary
window targeting across held modifiers, bounded UIA inspection/finalization, and annotation/marker
control ownership. `cargo check --tests`, formatting, and diff checks pass. Remaining Review,
anchor, and preview coverage stays pending in a later ordered M7 batch.

Depends on: M1-M6 production integration.

Coverage groups: keyboard serde/mapping/input/editor; schema/settings; recorder controls; semantic
keyboard and delays; mouse/repeats; window/process; clipboard privacy; UIA inspection;
markers/annotations; review/edit/undo; insertion anchors; preview/admission/cleanup.

Acceptance: fake seams avoid real SendInput/COM/process/clipboard/desktop dependence; obsolete
modifier rejection, direct insertion, hidden materialization, and session-option tests become stronger
behavioral contracts; no meaningful test is ignored, deleted, or trivialized.

Verification: precise discovered Nextest filters followed by broader MkMacro filters; all green.

## M8 — Full verification, commits, independent review, remediation

Status: `pending`

Depends on: M7.

Run `cargo fmt --all --check`, `cargo check`, `git diff --check`, and
`cargo nextest run --no-fail-fast`. Create coherent commits for keyboard/settings, semantic recording,
review/preview, and tests where practical. Perform an independent read-only review against the prompt
and this ledger, remediate substantive findings, rerun affected targets and the full suite when shared
runtime behavior changes, and leave the branch clean.

Acceptance: every applicable definition-of-done item is satisfied, no stale competing path remains,
full Nextest passes, review findings are resolved, and final reporting records actual checks/commits.
