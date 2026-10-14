# Codex Orchestrator Kickstart — Multi Launcher egui modernization + visual polish

You are the **orchestrator** for a single extended execution. Read `AGENTS.md` and all instructions before changing code. Use project `planner`, `implementer`, and `reviewer` agents. Read the complete companion document `MULTI_LAUNCHER_EGUI_UPGRADE_AND_THEME_PLAN.md` as an **execution contract**, not merely optional suggestions.

## Mission

Use the current head of the repository as a baseline. Upgrade egui/eframe from locked 0.27.2 to confirmed stable 0.36.2 (or verified later stable where justified), fix all compatibility issues, prove Windows behavior and performance parity, and **immediately continue to** modern, compact, Fluent-inspired theming and visual polish across the launcher and settings/plugin dialogs. It is **one back-to-back assignment with internally gated phases**; do not stop after compilation or request user confirmation between migration and theming.

## Hard rules

1. **Performance first**, functionality second only insofar as both are mandatory, visual appearance third. No reproducible meaningful latency/idle/GPU or cache regression accepted. No constant idle animations or repaint loops.
2. **Two phases, sequential:** Track A dependency/API/renderer migration with visual parity; Track B centralized theme, transactional preview, presets, typography/icons and surface-by-surface polish. Reviewable commits and baseline between phases.
3. **Behavior unchanged:** preserve all plugins, actions, prefix routing, hotkeys, radial tap/hold semantics, window position, focus, list/grid selection, native overlays and editor workflows. No unrelated refactor.
4. **Base on actual attached snapshot:** reconcile HEAD vs provided archive before branching; use branch `egui-modernization` by default; don't reset dirty work. Fetch/check branches only safely. Do not arbitrarily switch to master. Commit and push checkpoints where permitted.
5. **Active checkpoint cadence:** follow exact M0-A…M6-B checkpoints and commit meaningful validated increments. Prefix commits with checkpoint ID. Review after each checkpoint; do not batch the entire project into one commit.
6. **Do not falsify validation:** Linux compilation does not prove Windows-native hook/viewport behavior. Mark unrun checks, keep test fixtures/intended semantics, maintain actual benchmark logs.
7. **Avoid hot-path styling costs:** apply theme on state changes; no per-frame `set_visuals`/`set_fonts`/assets/texture decoding/settings reads; color-only changes must not regenerate list or notes geometry caches.
8. **Risk seams:** `eframe::App::update` to `logic`/`ui`, backend now Wgpu by default, `RootListGeometryCache` and notes font atlas identity, winit/native handles, `egui-toast`, `egui_commonmark`, secondary viewports and Win32 focus/hotkey interactions.
9. **Stop only at a genuine hard blocker:** investigate and fix before Track B. If migration is green, immediately continue through entire visual rollout. User selected a single continuous execution.
10. **Final response:** provide commits/SHAs, exact versions/backend, significant UI improvements, actual old vs migrated vs themed metrics, passing tests, missing tests, known risks, and rollback checkpoint SHA.

## Immediately execute

1. Planner: inventory code and dependencies and annotate concrete work items with file paths, test hooks and risks; compare real HEAD to snapshot.
2. Implementer: establish Windows performance/behavior baseline and branch; create `docs/plans/egui-modernization/` receipts; execute M0 and Track A sequentially.
3. Reviewer: inspect each migration checkpoint, especially caches/lifecycle/native/renderer. Fix regressions until migration-only M2-D gate passes.
4. Without asking the user again, implement Track B: compatible theme model, runtime style application, theme preview/presets, typography/icon standardization, launcher root/list/grid polish, dialogs/plugins, optional bounded effects if justified.
5. Reviewer: final full validation, repeated comparative benchmark, screenshots and evidence, tests/known issues; handoff and push if safe.

## Definition of done

Both phases completed in one continuous run; modernized dependencies; the launcher demonstrably looks better on root and common dialogs; persisted themes actually drive rendering; no new meaningful runtime performance regression; existing functionality preserved; checkpointed branch ready for review. If you cannot prove any part, state exactly what remains and why.