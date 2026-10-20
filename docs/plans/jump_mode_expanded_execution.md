# Jump Mode expanded coverage execution

Authority: `multi_launcher_jump_mode_full_coverage_plan.md` and current `AGENTS.md`.
Started 2026-10-20 from clean branch `jump`, HEAD `0cb3dabf148dc720ed560626c32ef596b5e8ff9a`; retain selected branch and existing architecture.

## Checkpoints

| Milestone | Status | Evidence / commits |
|---|---|---|
| M0-A | complete | Read active plan, repository instructions and configured roles; inspected Git, owner bridge, renderer, dashboard gaps and isolated runner. |
| M1 | in_progress | Configured planner preparing bounded owner-routing packets. |
| M2 File Search | pending | |
| M3 Snippets | pending | |
| M4 Clipboard History | pending | |
| M5 Dashboard gaps | pending | |
| M6 JSON / Regex / XML | pending | |
| M7 Mouse settings / gestures | pending | |
| M8 Rename / Replace | pending | |
| M9 Designers | pending | |
| M10 Integration / evidence | pending | |
| M11 Final gates | pending | |

## Initial ownership and constraints

- `JumpModeUi` / `JumpModeScope` own frame registry, focus handoff and input guards. Existing owners: Root, Settings, Notes, Todo, Timer, PasteSequence, MkMacro, Blocked. Actual visible top Middle layer and focused layer determine eligibility; unknown layers fail closed.
- `src/gui/render.rs` assembles `NavigationWindows`, tracks lifecycles and renders owned scopes. New families currently lack typed owner routing; exact window IDs and instance lifecycle are M1 prerequisites.
- Dashboard slot/header/body namespaces already exist. Todo, Scratchpad and Calendar have partial coverage; Pinned Commands and Command History lack row registration.
- Preserve default-off setting, real response geometry/clipping, combo provenance, distinct later activation and same-frame input discard. No second engine/hooks, synthesized canvas drags or hidden target enumeration.
- Clipboard, concealed snippets, source bodies and match previews must stay out of IDs, badges and evidence. Mutation confirmations must own foreground input separately.
- Existing fixture generator disables Paste Sequence Next/Cancel, disables clipboard observers and validates empty typed clipboard/snippet stores; runner uses isolated profile/evidence roots and owned-process receipts.

## Verification budget and evidence

One focused Nextest slice per checkpoint, at most one adjacent owner slice where justified. No full suite or Clippy during M0–M9. M10 at most two integration slices. M11 one full workspace Nextest (at most one repaired rerun), ordinary checks/docs/format/commits then final Clippy.

Native sessions: 0 / 12; images: 0 / 24. No native evidence yet. Use generated isolated data only; environment blocker receives one diagnostic and one retry.

## Coverage inventory

All eight inventories pending implementation; individual unsupported pointer-only, passive or foreign-popup controls will be recorded with their actual boundary. No universal coverage claim.

## Git and final gates

Parent owns commits and one best-effort normal push per checkpoint. No pushes attempted yet. Full Nextest and final Clippy not run.
